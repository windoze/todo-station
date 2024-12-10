# ToDo Station

一个基于Rust和Slint的简单日历和待办事项应用程序。

## 使用说明

1. 按照[getting-started guide](https://www.rust-lang.org/learn/get-started)安装Rust。
2. 用以下命令编译程序（注意: 在macOs下使用Python 3.13会导致程序无法编译，建议使用Python 3.12）：
    ```
    cargo build --release
    ```
3. 在下列位置创建一个名为`config.toml`的配置文件，文件内容参见[config.toml.example](config.toml.example)：
    * Windows: `C:\Users\<username>\AppData\Roaming\todo-station\`
    * MacOS: `/Users/<username>/Library/Application Support/todo-station/`
    * Linux: `/home/<username>/.config/todo-station/`
4. 运行程序
    ```
    cargo run --release
    ```

## 天气预报配置

天气信息来自[和风天气](https://www.qweather.com/)，需要注册账号并创建应用程序以获取项目ID和密钥ID。

1. 在[和风天气](https://id.qweather.com/#/login)注册账号。
2. 在["控制台 - 项目管理"](https://dev.qweather.com/docs/configuration/project-and-key/)中创建一个项目，并获得项目ID。
3. 在新创建的项目中选择"创建凭据"，然后按照[身份认证 JSON Web Token](https://dev.qweather.com/docs/authentication/jwt/)中的说明创建密钥。上传公钥后，获得密钥ID。
4. 将私钥中间的部分复制到`config.toml`中的`[weather] signing-key`字段。

## Outlook日历配置

首次运行程序会看到下列信息：
```
To sign in, use a web browser to open the page https://www.microsoft.com/link and enter the code ABCD1234 to authenticate.
```
打开浏览器访问`https://www.microsoft.com/link`并输入设备码（用实际的设备码替换`ABCD1234`），然后按照浏览器中的提示授权程序访问Outlook日历。

程序停止运行7天后已有的授权将会过期，需要重新授权。

Outlook日历及一些基本信息将会被授权给`00df9c7d-7b32-4e89-9e3e-834fff775318`这个Azure应用程序ID，如需使用其他的应用程序ID，请自行修改`[todo] app-id`，并可以参照下面的步骤创建新的Azure应用程序。

### 创建新的Azure应用程序

1. 参照[这篇文章](https://docs.microsoft.com/zh-cn/azure/active-directory/develop/quickstart-register-app)创建一个新的Azure应用程序。
2. 设置App权限，包含`Calendars.Read`、`offline_access`、`openid`、`profile`，注意这些是`Delegated`而非`Application`权限。
3. 在`Authentication`中允许`Allow public client flows`。

## 许可协议

* 本项目基于MIT许可协议发布 - 查看[LICENSE](LICENSE)文件了解更多信息。
* SourceHanSans-Regular字体基于SIL开源字体许可协议发布 - 查看[LICENSE.source-han-sans](LICENSE.source-han-sans)文件了解更多信息。
* 其他第三方资产根据其各自的许可协议发布。
