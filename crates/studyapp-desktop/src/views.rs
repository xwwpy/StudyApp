use crate::controls::Button;
use crate::controls::Checkbox;
use crate::{
    app::{EditorMode, Page, StudyApp},
    components::*,
};
use chrono::{Local, TimeZone, Utc};
use gpui_kit::component::{
    Disableable, IconName, Selectable, Sizable,
    button::ButtonVariants,
    input::{Input, Textarea},
    scroll::ScrollableElement,
    tooltip::Tooltip,
};
use gpui_kit::{prelude::FluentBuilder, *};
use studyapp_core::{Command, Review, SessionStatus, Snapshot};
use studyapp_desktop::presentation::{duration, stamp};

fn today_date() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}
fn day_of(time: i64) -> String {
    Local
        .timestamp_opt(time, 0)
        .single()
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}
fn optional_stamp(time: Option<i64>, empty: &str) -> String {
    time.map(stamp).unwrap_or_else(|| empty.to_owned())
}
fn review_tip(review: &Review) -> String {
    format!(
        "最近学习：{}\n上次复习：{}\n下次复习：{}",
        stamp(review.learned_at),
        optional_stamp(review.last_review_at, "尚未复习"),
        optional_stamp(review.next_review_at, "本轮已完成")
    )
}

impl StudyApp {
    pub fn today(&self, snapshot: &Snapshot, _window: &mut Window, cx: &mut Context<Self>) -> Div {
        let now = Utc::now().timestamp();
        let learned = self.read_models.today_learned_seconds();
        let due = self.read_models.due_count(now);
        let remaining = snapshot
            .timer
            .target_seconds
            .saturating_sub(snapshot.timer.active_seconds);
        let ratio = (snapshot.timer.active_seconds as f32
            / snapshot.timer.target_seconds.max(1) as f32)
            .clamp(0., 1.);
        let paused = snapshot.timer.paused;
        let mut content = column()
            .child(
                row()
                    .items_stretch()
                    .child(metric(
                        "今日已确认学习",
                        format!("{} 分钟", learned / 60),
                        "以你确认的记录为准",
                    ))
                    .child(metric(
                        "现在待复习",
                        format!("{due} 项"),
                        "也包含之前未完成的内容",
                    ))
                    .child(metric(
                        "我的笔记",
                        format!("{} 份", self.read_models.managed_note_count),
                        "导入后自动追踪文件更改",
                    )),
            )
            .child(
                card()
                    .bg(crate::theme::color(LIGHT))
                    .border_color(crate::theme::color(0xD2E7D8))
                    .child(
                        row()
                            .child(
                                column()
                                    .gap_1()
                                    .flex_1()
                                    .child(heading(if paused {
                                        "计时已暂停"
                                    } else {
                                        "专注当下，稍后回顾"
                                    }))
                                    .child(muted(format!(
                                        "本段 {} 分钟 · 下次询问倒计时",
                                        snapshot.timer.target_seconds / 60
                                    ))),
                            )
                            .child(
                                Button::new("pause")
                                    .label(if paused { "继续计时" } else { "暂停" })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.send(Command::SetPaused(!paused), cx)
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_3xl()
                            .font_family("Consolas")
                            .text_color(crate::theme::color(GREEN))
                            .child(duration(remaining)),
                    )
                    .child(
                        div()
                            .w_full()
                            .h(px(6.))
                            .bg(crate::theme::color(0xD3E5D8))
                            .rounded_full()
                            .child(
                                div()
                                    .w(relative(ratio))
                                    .h_full()
                                    .bg(crate::theme::color(GREEN))
                                    .rounded_full(),
                            ),
                    )
                    .child(
                        row()
                            .child(muted("结束后记录学习内容；超时未回答会记为未学习。"))
                            .child(div().flex_1())
                            .child(
                                Button::new("manual-top")
                                    .ghost()
                                    .icon(IconName::Plus)
                                    .label("手动补记")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_editor(EditorMode::Manual, window, cx)
                                    })),
                            ),
                    ),
            );
        if self.editor.is_some() {
            content = column()
                .child(self.editor_view(snapshot, cx))
                .child(content);
        }
        let pending = &self.read_models.pending_sessions;
        if !pending.is_empty() {
            content = content.child(card().child(heading("等待你的回答")).children(
                pending.iter().map(|&position| {
                    let s = &snapshot.sessions[position];
                    let id = s.id.clone();
                    row()
                        .child(
                            column()
                                .gap_1()
                                .flex_1()
                                .child(format!("{} – {}", stamp(s.started_at), stamp(s.ended_at)))
                                .child(muted(format!(
                                    "剩余 {} 秒 · {} 分钟活动时间",
                                    (s.deadline - now).max(0),
                                    s.active_seconds / 60
                                ))),
                        )
                        .child(
                            Button::new(SharedString::from(format!("answer-{id}")))
                                .primary()
                                .label("填写学习内容")
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.open_editor(EditorMode::Pending(id.clone()), window, cx)
                                })),
                        )
                }),
            ));
        }
        if self.read_models.managed_note_count == 0 {
            content = content.child(card().child(heading("从一份笔记开始")).child(muted("导入正在学习的 Markdown 文件。文件改动会形成版本历史，并在回顾时提供学习摘要。"))
                .child(Button::new("first-import").primary().label("选择 Markdown 笔记").on_click(cx.listener(|this, _, _, _| this.picker("import", None)))));
        }
        content.child(
            card()
                .child(
                    row()
                        .child(heading("最近的学习记录"))
                        .child(div().flex_1())
                        .child(
                            Button::new("all-records")
                                .ghost()
                                .label("查看全部")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.navigate(Page::Stats, window, cx);
                                })),
                        ),
                )
                .when(snapshot.sessions.is_empty(), |this| {
                    this.child(muted("尚无学习记录。完成一个时间段，或手动补记一次学习。"))
                })
                .children(
                    snapshot
                        .sessions
                        .iter()
                        .rev()
                        .take(8)
                        .map(|s| self.session_row(s, cx)),
                ),
        )
    }

    fn editor_view(&self, snapshot: &Snapshot, cx: &mut Context<Self>) -> Div {
        let Some(mode) = &self.editor else {
            return column();
        };
        let title = match mode {
            EditorMode::Pending(_) => "刚才，你学到了什么？",
            EditorMode::Manual => "补记一次学习",
            EditorMode::Edit(_) => "更正学习记录",
        };
        let mut view = card().border_color(crate::theme::color(GREEN)).child(
            row().child(heading(title)).child(div().flex_1()).child(
                Button::new("cancel-editor")
                    .ghost()
                    .small()
                    .label("收起")
                    .disabled(self.saving)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.editor = None;
                        cx.notify();
                    })),
            ),
        );
        if matches!(mode, EditorMode::Manual) {
            view = view.child(
                row()
                    .child(
                        column()
                            .flex_1()
                            .gap_2()
                            .child(muted("开始时间（本地）"))
                            .child(Input::new(&self.start)),
                    )
                    .child(
                        column()
                            .w(px(160.))
                            .gap_2()
                            .child(muted("学习分钟数"))
                            .child(Input::new(&self.minutes)),
                    ),
            );
        }
        view = view
            .child(Textarea::new(&self.summary).h(px(115.)))
            .child(muted(
                "关联学习笔记（可多选；摘要可以修改，只有提交后才算已学习）",
            ))
            .child(
                row().flex_wrap().children(
                    snapshot
                        .notes
                        .iter()
                        .filter(|note| !note.detached || self.chosen_notes.contains(&note.id))
                        .map(|note| {
                            let id = note.id.clone();
                            let selected = self.chosen_notes.contains(&id);
                            Checkbox::new(SharedString::from(format!("associate-{id}")))
                                .label(note.title.clone())
                                .checked(selected)
                                .on_click(cx.listener(move |this, checked, _, cx| {
                                    this.toggle_note(&id, *checked);
                                    cx.notify();
                                }))
                        }),
                ),
            );
        if !self.chosen_notes.is_empty() {
            view = view
                .child(muted(
                    "确认学习快照（每份笔记至少选择一个版本；时间为实际捕获时间，可多选或预览）",
                ))
                .child(
                    scroll_body("learning-versions", &self.learning_versions_scroll)
                        .max_h(px(220.))
                        .overflow_y_scroll()
                        .children(
                            self.read_models
                                .selected_version_positions(&self.chosen_notes)
                                .into_iter()
                                .map(|position| {
                                    let version = &snapshot.versions[position];
                                    let id = version.id.clone();
                                    let preview_id = id.clone();
                                    let note_id = version.note_id.clone();
                                    let title = self
                                        .read_models
                                        .note_position(&version.note_id)
                                        .and_then(|position| snapshot.notes.get(position))
                                        .map_or("笔记", |n| n.title.as_str());
                                    row()
                                        .child(
                                            Checkbox::new(SharedString::from(format!(
                                                "associate-version-{id}"
                                            )))
                                            .checked(self.chosen_versions.contains(&id))
                                            .label(format!(
                                                "{} · {}{} · {}",
                                                title,
                                                stamp(version.captured_at),
                                                if version.offline { " · 离线" } else { "" },
                                                version
                                                    .summary
                                                    .chars()
                                                    .take(70)
                                                    .collect::<String>()
                                            ))
                                            .on_click(cx.listener(move |this, checked, _, cx| {
                                                if *checked {
                                                    this.chosen_versions.insert(id.clone());
                                                } else {
                                                    this.chosen_versions.remove(&id);
                                                }
                                                cx.notify();
                                            })),
                                        )
                                        .child(
                                            Button::new(SharedString::from(format!(
                                                "preview-learning-{preview_id}"
                                            )))
                                            .ghost()
                                            .small()
                                            .label("预览")
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.show_version(
                                                    note_id.clone(),
                                                    preview_id.clone(),
                                                    window,
                                                    cx,
                                                )
                                            })),
                                        )
                                }),
                        ),
                );
        }
        let skip = match mode {
            EditorMode::Pending(id) => Some(Command::Skip {
                session_id: id.clone(),
            }),
            EditorMode::Edit(id) => Some(Command::EditSession {
                session_id: id.clone(),
                summary: self.summary.read(cx).value().to_string(),
                learned: false,
                note_ids: self.chosen_notes.iter().cloned().collect(),
                version_ids: None,
            }),
            EditorMode::Manual => None,
        };
        view.child(
            row()
                .child(
                    Button::new("save-learning")
                        .primary()
                        .label(if self.saving {
                            "正在保存…"
                        } else {
                            "保存学习记录"
                        })
                        .disabled(self.saving)
                        .on_click(cx.listener(|this, _, _, cx| this.save_editor(cx))),
                )
                .children(skip.map(|command| {
                    Button::new("skip-learning")
                        .ghost()
                        .label("这段时间没有学习")
                        .disabled(self.saving)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.send(command.clone(), cx);
                            this.editor = None;
                        }))
                }))
                .child(muted("提交后自动生成复习计划")),
        )
    }

    fn session_row(&self, session: &studyapp_core::Session, cx: &mut Context<Self>) -> Div {
        let (label, color) = match session.status {
            SessionStatus::Learned => ("已学习", LIGHT),
            SessionStatus::Missed => ("未学习", 0xF4ECE6),
            SessionStatus::Pending => ("待回答", 0xFFF3CF),
        };
        let mode = if session.status == SessionStatus::Pending {
            EditorMode::Pending(session.id.clone())
        } else {
            EditorMode::Edit(session.id.clone())
        };
        row()
            .py_3()
            .border_b_1()
            .border_color(crate::theme::color(BORDER))
            .child(pill(label, color))
            .child(
                column()
                    .gap_1()
                    .flex_1()
                    .min_w_0()
                    .child(div().child(if session.summary.is_empty() {
                        "尚未填写学习内容".into()
                    } else {
                        session.summary.chars().take(160).collect::<String>()
                    }))
                    .child(muted(format!(
                        "{}  ·  {} 分钟  ·  {} 份关联笔记 · {} 个快照",
                        stamp(session.started_at),
                        session.active_seconds / 60,
                        session.note_ids.len(),
                        session.version_ids.len()
                    ))),
            )
            .child(
                Button::new(SharedString::from(format!("edit-{}", session.id)))
                    .ghost()
                    .small()
                    .label("编辑")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_editor(mode.clone(), window, cx)
                    })),
            )
    }

    pub fn reviews(&self, snapshot: &Snapshot, cx: &mut Context<Self>) -> Div {
        let now = Utc::now().timestamp();
        let today = today_date();
        let mut content = column().child(
            row().children(
                ["待复习与逾期", "今日已完成", "全部计划"]
                    .into_iter()
                    .enumerate()
                    .map(|(i, label)| {
                        Button::new(("review-filter", i))
                            .label(label)
                            .selected(self.review_filter == i)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.review_filter = i;
                                cx.notify();
                            }))
                    }),
            ),
        );
        let reviews = self.read_models.review_positions(self.review_filter, now);
        if reviews.is_empty() {
            content = content.child(empty(
                "当前没有复习任务",
                "学习确认后会自动安排复习。到期后勾选完成，下一次时间随之更新。",
            ));
        }
        for &position in reviews {
            let review = &snapshot.reviews[position];
            let id = review.id.clone();
            let stage = review.stage;
            let tip = review_tip(review);
            let due = review.next_review_at.is_some_and(|t| t <= now);
            let done_today = self.read_models.review_completed_today(&id);
            let complete = review.next_review_at.is_none();
            let mut item = card()
                .id(SharedString::from(format!("review-card-{id}")))
                .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                .child(
                    row()
                        .child(
                            Checkbox::new(SharedString::from(format!("review-check-{id}")))
                                .checked(complete || (done_today && !due))
                                .disabled(!due && !done_today)
                                .accessibility_label("标记复习完成或撤销")
                                .on_click(cx.listener(move |this, checked, _, cx| {
                                    this.send(
                                        if *checked {
                                            Command::CompleteReview {
                                                review_id: id.clone(),
                                                expected_stage: stage,
                                            }
                                        } else {
                                            Command::UndoReview {
                                                review_id: id.clone(),
                                            }
                                        },
                                        cx,
                                    );
                                })),
                        )
                        .child(
                            column()
                                .flex_1()
                                .gap_1()
                                .child(heading(review.title.clone()))
                                .child(muted(format!(
                                    "学习于 {} · 已完成 {}/{} 次复习",
                                    stamp(review.learned_at),
                                    review.stage,
                                    review.intervals.len()
                                ))),
                        )
                        .child(pill(
                            if complete {
                                "本轮已完成"
                            } else if due {
                                if review.next_review_at.is_some_and(|t| day_of(t) != today) {
                                    "已逾期"
                                } else {
                                    "现在复习"
                                }
                            } else {
                                "已安排"
                            },
                            if due { 0xFFF3CF } else { LIGHT },
                        )),
                )
                .child(
                    row()
                        .flex_wrap()
                        .children(review.intervals.iter().enumerate().map(|(i, days)| {
                            pill(
                                format!(
                                    "{} · {} 天",
                                    if i < review.stage { "✓" } else { "○" },
                                    days
                                ),
                                if i < review.stage { LIGHT } else { BG },
                            )
                        })),
                )
                .child(muted(format!(
                    "上次：{}    下次：{}",
                    optional_stamp(review.last_review_at, "尚未复习"),
                    optional_stamp(review.next_review_at, "本轮已完成")
                )));
            let review_id = review.id.clone();
            if review.last_review_at.is_some() {
                item = item.child(
                    Button::new(SharedString::from(format!("undo-{review_id}")))
                        .ghost()
                        .small()
                        .label("撤销最近一次复习")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.send(
                                Command::UndoReview {
                                    review_id: review_id.clone(),
                                },
                                cx,
                            )
                        })),
                );
            }
            content = content.child(item);
        }
        content.child(muted(
            "默认间隔 1、3、7、14、30 天；每次完成后，从实际完成时间计算下一次复习。",
        ))
    }

    pub fn statistics(&self, snapshot: &Snapshot, cx: &mut Context<Self>) -> Div {
        let days = self.read_models.recent_days(self.stats_days);
        let total = self.read_models.learned_seconds;
        let learned = self.read_models.learned_count;
        let max = days
            .iter()
            .map(|d| d.learned_seconds)
            .max()
            .unwrap_or(0)
            .max(60);
        let mut content = column()
            .child(
                row()
                    .items_stretch()
                    .child(metric(
                        "累计学习",
                        format!("{:.1} 小时", total as f64 / 3600.),
                        "所有已确认学习记录",
                    ))
                    .child(metric(
                        "学习记录",
                        format!("{learned} 次"),
                        "未回答时间段不计入",
                    ))
                    .child(metric(
                        "累计复习",
                        format!("{} 次", snapshot.review_events.len()),
                        "已完成且未撤销的复习",
                    )),
            )
            .child(
                card()
                    .child(
                        row()
                            .child(heading("每日学习时长"))
                            .child(div().flex_1())
                            .children([7, 30, 90].into_iter().map(|days| {
                                Button::new(("stats-days", days))
                                    .small()
                                    .ghost()
                                    .selected(self.stats_days == days)
                                    .label(format!("{days} 天"))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.stats_days = days;
                                        cx.notify();
                                    }))
                            })),
                    )
                    .child(muted(format!("最高 {} 分钟 / 天 · 悬停查看明细", max / 60)))
                    .child(
                        row()
                            .id("study-bars")
                            .relative()
                            .track_scroll(&self.statistics_scroll)
                            .horizontal_scrollbar(&self.statistics_scroll)
                            .overflow_x_scroll()
                            .pb_3()
                            .items_end()
                            .gap_2()
                            .h(px(200.))
                            .children(days.iter().enumerate().map(|(i, stat)| {
                                let height =
                                    (stat.learned_seconds as f32 / max as f32 * 145.).max(2.);
                                let tip = format!(
                                    "{}\n学习 {} 分钟 · {} 次记录\n未学习 {} 段 · 复习 {} 次",
                                    stat.date,
                                    stat.learned_seconds / 60,
                                    stat.learned_count,
                                    stat.missed_count,
                                    stat.review_count
                                );
                                column()
                                    .id(("bar", i))
                                    .w(px(24.))
                                    .flex_shrink_0()
                                    .gap_2()
                                    .items_center()
                                    .justify_end()
                                    .h_full()
                                    .tooltip(move |window, cx| {
                                        Tooltip::new(tip.clone()).build(window, cx)
                                    })
                                    .child(div().w(px(18.)).h(px(height)).rounded_t_md().bg(
                                        crate::theme::color(if stat.learned_seconds > 0 {
                                            GREEN
                                        } else {
                                            BORDER
                                        }),
                                    ))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(crate::theme::color(MUTED))
                                            .child(stat.date[8..].to_owned()),
                                    )
                            })),
                    ),
            );
        let heat = self.read_models.calendar();
        let maximum = heat
            .iter()
            .map(|d| d.review_count)
            .max()
            .unwrap_or(1)
            .max(1);
        content = content.child(
            row()
                .items_stretch()
                .child(
                    card()
                        .flex_1()
                        .child(heading("最近 13 周的积累"))
                        .child(muted("颜色越深，当天确认的学习时间越长"))
                        .child(row().gap_1().children(heat.chunks(7).enumerate().map(
                            |(week, chunk)| {
                                column().gap_1().children(chunk.iter().enumerate().map(
                                    move |(day, stat)| {
                                        let color = match stat.learned_seconds {
                                            0 => BORDER,
                                            1..=1799 => 0xD1E6D6,
                                            1800..=3599 => 0x9AC9AC,
                                            3600..=7199 => 0x55A580,
                                            _ => GREEN,
                                        };
                                        let tip = format!(
                                            "{} · {} 分钟",
                                            stat.date,
                                            stat.learned_seconds / 60
                                        );
                                        div()
                                            .id(("heat", week * 7 + day))
                                            .size(px(18.))
                                            .rounded_sm()
                                            .bg(crate::theme::color(color))
                                            .tooltip(move |w, cx| {
                                                Tooltip::new(tip.clone()).build(w, cx)
                                            })
                                    },
                                ))
                            },
                        ))),
                )
                .child(
                    card()
                        .flex_1()
                        .child(heading("最近两周的复习"))
                        .child(muted("每天完成的复习次数"))
                        .child(row().items_end().gap_2().h(px(140.)).children(
                            heat.iter().skip(77).enumerate().map(|(i, stat)| {
                                let tip = format!("{} · {} 次复习", stat.date, stat.review_count);
                                column()
                                    .id(("review-bar", i))
                                    .flex_1()
                                    .h_full()
                                    .justify_end()
                                    .items_center()
                                    .gap_2()
                                    .tooltip(move |w, cx| Tooltip::new(tip.clone()).build(w, cx))
                                    .child(div().text_xs().child(stat.review_count.to_string()))
                                    .child(
                                        div()
                                            .w(px(13.))
                                            .h(px((stat.review_count as f32 / maximum as f32
                                                * 85.)
                                                .max(2.)))
                                            .rounded_t_md()
                                            .bg(crate::theme::color(0xB39869)),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(crate::theme::color(MUTED))
                                            .child(stat.date[8..].to_owned()),
                                    )
                            }),
                        )),
                ),
        );
        let record_date = self.record_date.read(cx).value();
        let record_date = record_date.trim();
        let sessions = self
            .read_models
            .record_positions(record_date, self.record_filter);
        content.child(
            card()
                .child(
                    row()
                        .child(heading(format!("全部学习记录 · {} 条", sessions.len())))
                        .child(
                            div()
                                .id("record-date-filter")
                                .w(px(210.))
                                .child(Input::new(&self.record_date).cleanable(true)),
                        )
                        .child(div().flex_1())
                        .children(
                            ["全部", "已学习", "未学习", "待回答"]
                                .into_iter()
                                .enumerate()
                                .map(|(i, label)| {
                                    Button::new(("record-filter", i))
                                        .ghost()
                                        .small()
                                        .selected(self.record_filter == i)
                                        .label(label)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.record_filter = i;
                                            this.record_limit = 50;
                                            cx.notify();
                                        }))
                                }),
                        ),
                )
                .when(sessions.is_empty(), |c| {
                    c.child(muted("当前筛选下还没有记录。"))
                })
                .children(
                    sessions
                        .iter()
                        .take(self.record_limit)
                        .map(|&position| self.session_row(&snapshot.sessions[position], cx)),
                )
                .when(sessions.len() > self.record_limit, |c| {
                    c.child(Button::new("more-records").label("再显示 50 条").on_click(
                        cx.listener(|this, _, _, cx| {
                            this.record_limit += 50;
                            cx.notify();
                        }),
                    ))
                }),
        )
    }

    pub fn settings(
        &self,
        snapshot: &Snapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        use crate::settings_ui::SettingsSection;
        let compact = window.viewport_size().width < px(1120.);
        let section = self.preferences_ui.section;
        let content = column().min_w_0().w_full().max_w(px(960.)).flex_shrink_0().gap_3().child(self.preference_controls(cx))
            .children((section == SettingsSection::Storage).then(|| card().child(heading("数据保存"))
                .child(row().child(pill("✓ 本地资料库", LIGHT)).child(muted("可在下方配置笔记文件同步")))
                .child(muted(studyapp_desktop::presentation::display_path(&snapshot.data_path)))
                .child(row().flex_wrap().child(Button::new("migrate-library")
                    .disabled(self.storage_busy || self.note_saving || self.sync_panel.read(cx).status.busy)
                    .label("更改保存位置并迁移")
                    .on_click(cx.listener(|this, _, window, cx| this.choose_storage(window, cx))))
                    .child(Button::new("switch-library").disabled(self.storage_busy).label("打开其他资料库").on_click(cx.listener(|this, _, _, cx| this.settings_picker("library", None, cx))))
                    .child(Button::new("backup").label("导出备份").on_click(cx.listener(|this, _, _, cx| this.settings_picker("backup", None, cx))))
                    .child(Button::new("restore").label("从备份恢复").on_click(cx.listener(|this, _, _, cx| this.settings_picker("restore", None, cx)))))
                .child(muted("迁移会复制笔记、历史、草稿和同步设置到新的空目录，保留原资料，正常启动时记住新位置。恢复备份只恢复学习资料，保留当前配置。使用 --data-dir 的独立运行不修改默认位置。"))))
            .children((section == SettingsSection::Storage).then(|| self.storage_view(cx)))
            .children((section == SettingsSection::Sync).then(|| self.sync_panel.clone()))
            .children((section == SettingsSection::AboutAccount).then(|| self.update_panel.clone()))
            .children((section == SettingsSection::AboutAccount).then(|| card().child(heading("账户与微信登录"))
                .child(row().child(pill("本地访客", LIGHT)).child(Button::new("wechat-disabled").label("微信登录 · 尚未连接服务").disabled(true)))
                .child(muted("无需登录即可使用本地功能及自己的文件同步服务。微信登录仍保留后续接入。"))))
            .children((section == SettingsSection::AboutAccount).then(|| card().child(heading("文件历史与隐私"))
                .child(muted("新建笔记保存在资料库 notes 目录，保存时自动生成版本；外部导入文件保持只读。运行时约每 3 秒检查文件改动，快速连续保存会合并为稳定版本。"))
                .child(muted("离线期间只能比较上次快照与当前内容，无法还原中间的每一次保存。文件变化用于建议摘要，必须由你确认才会记为学习。"))
                .child(muted("笔记预览支持图片，原始 HTML 不会执行；学习时长来自你的确认，不代表对实际专注的自动测量。"))));
        let transitioning = !cx.reduce_motion()
            && self
                .preferences_ui
                .section_changed_at
                .is_some_and(|changed| changed.elapsed() < std::time::Duration::from_millis(140));
        let content = if transitioning {
            content
                .with_animation(
                    (
                        "settings-section-transition",
                        self.preferences_ui.section_revision as usize,
                    ),
                    Animation::new(std::time::Duration::from_millis(120)),
                    |content, progress| content.opacity(0.94 + 0.06 * progress),
                )
                .into_any_element()
        } else {
            content.into_any_element()
        };
        let workspace = if compact {
            column()
        } else {
            row().items_stretch()
        };
        column()
            .h_full()
            .min_h_0()
            .min_w_0()
            .gap_2()
            .child(
                div()
                    .id("settings-save-header")
                    .test_support()
                    .flex_shrink_0()
                    .child(self.settings_header(cx)),
            )
            .child(
                workspace
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .gap_3()
                    .child(
                        self.settings_navigation(compact, cx)
                            .id("settings-category-navigation")
                            .test_support()
                            .flex_shrink_0(),
                    )
                    .child(if section == SettingsSection::Shortcuts {
                        self.shortcut_controls(cx).into_any_element()
                    } else if section == SettingsSection::Appearance {
                        self.appearance_surface(cx).into_any_element()
                    } else {
                        scroll_body("settings-section-scroll", &self.settings_scroll)
                            .test_support()
                            .flex_1()
                            .min_h_0()
                            .min_w_0()
                            .pr_4()
                            .pb_4()
                            .child(content)
                            .into_any_element()
                    }),
            )
    }
}
