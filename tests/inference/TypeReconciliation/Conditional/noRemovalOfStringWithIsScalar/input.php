<?php
$a = rand(0, 1) ? "hello" : "goodbye";

if (!is_scalar($a)) {
    exit;
}
