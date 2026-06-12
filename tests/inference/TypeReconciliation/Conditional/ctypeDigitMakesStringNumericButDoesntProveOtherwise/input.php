<?php
function bar(string $m): void
{
    if (is_numeric($m)) {
        if (ctype_digit($m)) {
            echo "I'm an all-digit numeric-string";
        } else {
            echo "I'm not an all-digit numeric-string";
        }
    }
}
