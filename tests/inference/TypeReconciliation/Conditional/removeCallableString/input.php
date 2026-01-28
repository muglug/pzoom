<?php
$s = rand(0,1) ? "strlen" : [1];
if (!is_callable($s)) {
    array_pop($s);
}