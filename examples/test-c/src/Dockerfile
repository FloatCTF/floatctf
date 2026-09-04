FROM php:8.2-apache-bookworm
LABEL Author="your_name <your_email@example.com>"

COPY flag /flag
COPY entrypoint.sh /entrypoint.sh
COPY index.php /var/www/html/index.php
RUN chmod +x /entrypoint.sh

# 运行时端口契约来自 meta.toml [docker].port（EXPOSE 仅示意）
EXPOSE 80
WORKDIR /var/www/html

ENTRYPOINT ["/entrypoint.sh"]
CMD ["apache2-foreground"]
