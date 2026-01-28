<?php
function bar(string $s) : string {
    $parsed = parse_url($s);

    return $parsed["host"];
}
