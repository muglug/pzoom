<?php
function test(): int {
    if (rand(0, 1) || ($a = rand(0, 10)) === 0) {
        return 0;
    }

    return $a;
}

function test2(?string $comment): ?string {
    if ($comment === null || preg_match("/.*/", $comment, $match) === 0) {
        return null;
    }

    return $match[0];
}