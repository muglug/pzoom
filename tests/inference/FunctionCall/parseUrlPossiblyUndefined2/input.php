<?php
function bag(string $s) : string {
    $parsed = parse_url($s);

    if (is_string($parsed["host"] ?? false)) {
        return $parsed["host"];
    }

    return "";
}
