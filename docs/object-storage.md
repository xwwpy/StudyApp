# 图片对象存储

图片云端目标独立于 Markdown 笔记同步。选择“云端”或“本地与云端”后，在同一页面填写通用对象存储参数，或者明确选择复用当前笔记同步的 WebDAV/文件夹/NAS 目标。未保存的字段不会用于上传。

通用对象存储使用 **S3 兼容协议、AWS Signature Version 4**。公开参数为 Endpoint、Bucket、Region、Access Key ID、对象前缀和寻址方式；Secret 单独使用 Windows 当前用户 DPAPI 保护，不进入笔记、前端编辑器或公开配置。Endpoint 使用 HTTPS，禁止包含账户密码、查询参数或对象路径；按服务商文档填写 Region 与 Endpoint。支持虚拟主机和路径两种寻址，服务器重定向不自动跟随。

接口范围仅为图片资产读取、创建和连接检查；不提供远端删除、存储桶创建、权限修改、multipart、大文件或 Markdown 双向同步。每张图片最多 10 MiB，网络请求有连接和总时限。创建对象使用 `If-None-Match: *`，已有对象只有内容相同才视作重试成功；不同内容不会被覆盖。

S3 兼容并不意味着每个对象存储服务都实现相同功能。应使用服务商的 S3 兼容入口，并检查其 SigV4、条件 PUT 与寻址支持。例如 OSS 的 S3 兼容能力要求虚拟主机寻址，部分账户需要启用兼容认证；不能将 OSS 原生签名与 AWS SigV4 混用。本实现不增加任何厂商专用按钮，也未使用真实服务商账户完成互通认证。

“云端”仍保留本地离线缓存及完整暂存文件，上传失败不删除本地图片和用户源图片。“本地与云端”表示主动保留本地文件并上传副本。变更存储模式或目标不会自动批量移动、清理或删除旧目录；使用“重试图片上传”将已有本地资产按稳定对象键上传至当前已保存目标。连接测试检查可访问性，实际上传权限由上传结果确认。

自定义本地图片目录使用“选择文件夹”，选择结果仅修改设置草稿，仍需“保存全部设置”。取消选择无变化；重置、资料库切换或更新的选择请求会使旧对话框结果失效。

协议依据：

- [AWS SigV4 规范](https://docs.aws.amazon.com/AmazonS3/latest/developerguide/sig-v4-header-based-auth.html)
- [S3 条件创建](https://docs.aws.amazon.com/AmazonS3/latest/userguide/conditional-writes.html)
- [S3 寻址方式](https://docs.aws.amazon.com/AmazonS3/latest/userguide/VirtualHosting.html)
- [OSS S3 兼容范围](https://www.alibabacloud.com/help/en/oss/developer-reference/compatibility-with-amazon-s3)
