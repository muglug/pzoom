<?php
$f = rand(0, 1) ? "strlen" : 1.1;
if (is_callable($f)) {
    Closure::fromCallable($f);
}