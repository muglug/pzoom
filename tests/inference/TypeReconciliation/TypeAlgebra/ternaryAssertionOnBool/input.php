<?php
function test(string|object $s, bool $b) : string {
    if (!$b || is_string($s)) {
        return $b ? $s : "";
    }
    return "";
}
