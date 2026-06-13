<?php

$str = $argv[1] ?? '';
if (empty($str) || strlen($str) < 3) {
    exit(1);
}

echo $str;
