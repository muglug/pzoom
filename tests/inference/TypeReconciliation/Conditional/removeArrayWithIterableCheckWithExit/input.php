<?php
$a = rand(0,1) ? "foo" : [1];
if (is_iterable($a)) {
    return;
}
strlen($a);