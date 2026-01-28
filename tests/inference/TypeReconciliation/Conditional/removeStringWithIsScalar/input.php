<?php
$a = rand(0, 1) ? "hello" : null;

if (is_scalar($a)) {
    exit;
}