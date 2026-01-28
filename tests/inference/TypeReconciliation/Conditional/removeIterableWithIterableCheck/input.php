<?php
/** @var string|iterable */
$s = rand(0,1) ? "foo" : [1];
if (!is_iterable($s)) {
    strlen($s);
}