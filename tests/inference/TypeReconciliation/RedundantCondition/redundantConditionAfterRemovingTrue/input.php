<?php
$s = rand(0, 1) ? rand(0, 5) : true;

if ($s !== true) {
    if (is_int($s)) {}
}
