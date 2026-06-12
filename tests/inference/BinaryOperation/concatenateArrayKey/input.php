<?php

/** @param array<array-key, string> $arr */
function f(array $arr): void
{
    foreach ($arr as $k => $v) {
        echo 'x: ' . $k . $v;
    }
}
