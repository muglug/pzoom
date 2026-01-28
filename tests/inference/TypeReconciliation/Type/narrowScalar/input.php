<?php
/** @var scalar $s */
$s = 1;

if (!is_int($s) && !is_bool($s) && !is_float($s)) {
    strlen($s);
}