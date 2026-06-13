<?php
$arr_or_string = [];

if (rand(0, 1)) {
  $arr_or_string = "hello";
}

/** @return void **/
function foo(string $s) {}

if (!empty($arr_or_string)) {
    foo($arr_or_string);
}
