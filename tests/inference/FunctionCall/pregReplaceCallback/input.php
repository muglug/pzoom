<?php
function foo(string $s) : string {
    return preg_replace_callback(
        '/<files (psalm-version="[^"]+") (?:php-version="(.+)">\n)/',
        /** @param string[] $matches */
        function (array $matches) : string {
            return $matches[1];
        },
        $s
    );
}
