# Dockerでローカル評価する

SynapseGit LP Studioは、Windows + WSL2環境を汚さずに試すための
Linux amd64 Docker local-build profileを提供します。配布済みimageを取得する方式では
なく、このrepositoryのsourceから利用者自身のPCでbuildします。

> **配布境界:** SynapseGit v0.4.0のlicenseは、全Rights Holdersの別途書面許諾なしに
> container image、GitHub Package、GitHub Release、downloadable binaryを公開する
> 権限を付与していません。LP Studio自身の一般license grantも未記録です。この手順は
> local non-commercial evaluation専用であり、production、外部納品、再配布、hostingを
> 許可するものではありません。

## 対応範囲

| 項目 | 現在の範囲 |
| --- | --- |
| Container | Linux amd64 |
| Host | Docker Engine 28以降。WindowsではDocker Desktop + WSL2 |
| Browser | Chromium系。Windows実機のmanual support evidenceはpending |
| Network | Editor `127.0.0.1:4173`、Preview `*.localhost:4174` |
| State | Docker named volume。Windows filesystem bindは非対応 |
| Image配布 | なし。local buildのみ |

Docker Engine 28より前には、localhostへpublishしたportへ同一L2の別hostから到達できる
既知の問題があります。Dockerの
[port publishing documentation](https://docs.docker.com/engine/network/port-publishing/)
に従い、Engine 28以降だけをこのprofileの対象にします。

## 1. WSL integrationを有効にする

Docker Desktopを起動し、`Settings > Resources > WSL Integration`で使用中の
distributionを有効にして`Apply`します。Docker公式の
[WSL 2手順](https://docs.docker.com/desktop/features/wsl/)
にも同じ設定が記載されています。

WSL terminalで確認します。

```bash
docker version
docker compose version
```

`docker`が見つからない、またはWSL integrationを有効にするよう表示される場合は、
先にDocker Desktopの設定を直します。WSLへ別のDocker Engineを重ねてinstallしません。

## 2. 起動する

repositoryは`/home/USER/...`のようなWSL native filesystemへ置き、そのrootで次を
実行します。source/build contextを`/mnt/c/...`へ置く構成は、I/O性能とLinux file
semanticsの評価対象外です。

```bash
docker compose up --build
```

初回はNode/Rust dependencyとアプリをcontainer buildします。起動後、Windows側の
Chromiumで次を開きます。

```text
http://127.0.0.1:4173
```

`Ctrl+C`で停止します。containerは停止しますが、Project stateは
`synapsegit-lp-studio_studio-state` named volumeに残ります。
`docker compose ps`のhealthは、container内からEditor/Preview両方のroleを検査します。

backgroundで使う場合は次の通りです。

```bash
docker compose up --build -d
docker compose logs -f studio
docker compose down
```

`docker compose down`はcontainer/networkだけを除去し、state volumeを保持します。
`docker compose down --volumes`は全Project stateを削除するため、初期化する意図がある
場合以外は実行しません。

## 3. 既存のbuild済みLPを取り込む

取り込み元は`index.html`を持つ実在directoryで、stateとは別のpathにします。
WSL内の絶対pathを指定します。

```bash
export LP_STUDIO_IMPORT_PATH=/home/USER/path/to/built-lp
docker compose -f compose.yaml -f compose.import.yaml up --build
```

Import directoryはcontainerへread-onlyでmountされます。Composeは存在しないhost pathを
自動作成しません。LP Studioは確認後にmanaged stateへcopyし、取り込み元を変更しません。

## 4. OpenAI providerを使う

credentialはimage、Compose file、container environment metadataへ保存しません。
専用Docker named volumeへ標準入力から格納し、アプリcontainerにはread-only mountします。
key自体をcommand lineやhost側の環境変数へ書かない手順です。

```bash
docker compose build
./scripts/init-docker-openai-secret.sh
export LP_STUDIO_OPENAI_MODEL='gpt-5.4-mini'
docker compose -f compose.yaml -f compose.openai.yaml up
```

LP Studioは`/run/secrets/openai-api-key`からkeyを読みます。`docker inspect`のenvironment
にはkeyの値を置きません。fake providerだけを使う場合はoverlayと環境変数を使いません。
専用volumeはcontainer停止後も残るため、credentialが不要になった時点で明示的に削除します。

```bash
docker compose -f compose.yaml -f compose.openai.yaml down
docker volume rm synapsegit-lp-studio-openai-secret
unset LP_STUDIO_OPENAI_MODEL
```

volume削除前に同じOpenAI overlayを再起動すればkeyを再利用できます。Docker Desktopの
volume storage、diagnostics、組織のendpoint security policyも含め、credential管理は
利用者の責任です。

## 5. 評価環境を片付ける

Node、pnpm、Rust dependencyはWSLへinstallされません。Docker Desktop内にはlocal image、
Project state volume、任意のcredential volume、build cacheが残ります。Project dataが
不要であることを確認してから、LP Studio固有のartifactを次の順で削除できます。

```bash
docker compose down --volumes
docker volume rm synapsegit-lp-studio-openai-secret  # 作成した場合だけ
docker image rm synapsegit-lp-studio:local
```

`down --volumes`はProject stateを復元不能に削除します。Docker build cacheは他projectと
共有されるため、この手順からはpruneしません。必要な場合はDocker Desktopのstorage管理で
影響範囲を確認してから別途整理します。

## Security boundary

provided ComposeはEditorとPreviewをhostのIPv4 loopbackだけへ公開します。Dockerは
host IPを省略したport publishを全interfaceへ公開するため、次のような実行はsupportしません。

```text
docker run -p 4173:4173 -p 4174:4174 ...
```

port番号の変換、LAN公開、reverse proxy、alternate hostnameもsupportしません。
Previewのscoped Host、CSP、browser originがexternal portを含むため、host/container portは
4173/4174で同一である必要があります。

Containerはnon-root UID 65532、read-only root filesystem、全capability drop、
`no-new-privileges`、bounded tmpfsで動作します。Project stateはLinux filesystem上のnamed
volumeへ置きます。SQLite lockingとowner-only modeを保つため、`/mnt/c`等のWindows pathを
stateへbind mountしません。

ImageにはTLS接続用CA bundle、LP Studioの未解決evaluation notice、exact SynapseGit
v0.4.0 license、build時に解決したdependencyのlicense inventoryと検出できたlicense/notice
本文を同梱します。これは公開再配布の許諾や正式なlegal clearanceを意味しません。

## Troubleshooting

### Port is already allocated

WindowsまたはWSLで4173/4174を使う別processを停止します。このprofileはorigin bindingの
ためport remappingをsupportしません。

### Editorは開くがPreviewが表示されない

Windows側で次が`status: ok`を返すか確認します。

```text
http://127.0.0.1:4174/health
http://localhost:4174/health
```

VPN、proxy、endpoint securityが`*.localhost`を外部DNSへ送っていないか確認します。
`lvh.me`等の外部DNS fallbackへ切り替えるとPreview originとprivacy境界が変わるため、
このprofileでは使用しません。

### state directoryのpermission error

Composeのnamed volume以外をstate rootへmountしていないことを確認します。既存volumeを
手動で所有者変更した場合は、そのvolumeをbackupしてから別volumeで再評価します。
