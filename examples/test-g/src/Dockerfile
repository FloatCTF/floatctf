FROM php:8.2-apache-bookworm

ARG DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        openssh-server \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /run/sshd \
    && printf '%s\n' \
        'PasswordAuthentication yes' \
        'PermitRootLogin no' \
        > /etc/ssh/sshd_config.d/floatctf.conf

COPY index.php /var/www/html/index.php
# AWDP 契约：/flag.php 按 FLAG env 返回本实例 flag（平台注入 FLAG）。
COPY flag.php /var/www/html/flag.php
COPY entrypoint.sh /entrypoint.sh

RUN chmod 0755 /entrypoint.sh && chmod 0644 /var/www/html/*.php

WORKDIR /var/www/html

EXPOSE 22 80

ENTRYPOINT ["/entrypoint.sh"]
CMD ["apache2-foreground"]
