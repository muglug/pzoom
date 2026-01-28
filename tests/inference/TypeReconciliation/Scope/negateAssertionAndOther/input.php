<?php
$a = rand(0, 10) ? "hello" : null;

if (rand(0, 10) > 1 && is_string($a)) {
    throw new \Exception("bad");
}