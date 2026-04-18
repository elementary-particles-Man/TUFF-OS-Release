#!/bin/bash
set -e

# KAIRO Standalone Installer
# Usage: sudo ./install.sh

if [[ $EUID -ne 0 ]]; then
   echo "このスクリプトは root 権限で実行する必要があります (sudo ./install.sh)"
   exit 1
fi

echo "--- KAIRO Standalone インストールを開始します ---"

# ディレクトリ作成
mkdir -p /etc/kairo
mkdir -p /var/log/kairo

# バイナリのインストール
echo "バイナリをインストール中..."
cp ./bin/kairo-daemon /usr/bin/kairo-daemon
chmod 755 /usr/bin/kairo-daemon

# 設定ファイルの配置
if [ ! -f /etc/kairo/KAIRO-ACL.txt ]; then
    echo "初期設定ファイルを配置中..."
    cp ./KAIRO-ACL.txt /etc/kairo/KAIRO-ACL.txt
    chmod 644 /etc/kairo/KAIRO-ACL.txt
fi

# systemd ユニットファイルの作成
echo "systemd サービスを登録中..."
cat <<EOF > /etc/systemd/system/kairo-daemon.service
[Unit]
Description=KAIRO AI-Proxy Daemon (Standalone, Ethics-free)
After=network.target

[Service]
Type=simple
ExecStart=/usr/bin/kairo-daemon
Restart=always
RestartSec=5
WorkingDirectory=/etc/kairo
Environment=KAIRO_ACL_PATH=/etc/kairo/KAIRO-ACL.txt
Environment=RUST_LOG=info
StandardOutput=append:/var/log/kairo/daemon.log
StandardError=append:/var/log/kairo/daemon.log

[Install]
WantedBy=multi-user.target
EOF

# サービスの有効化と起動
systemctl daemon-reload
systemctl enable kairo-daemon
systemctl restart kairo-daemon

echo "--- インストール完了 ---"
echo "ステータス確認: systemctl status kairo-daemon"
echo "ログ確認: tail -f /var/log/kairo/daemon.log"
echo "設定変更: vi /etc/kairo/KAIRO-ACL.txt (変更後 systemctl restart kairo-daemon)"
