<?php
$arr = [];
function fooFoo(array &$v): void {}
$function = "fooFoo";
$function($arr);
if ($arr) {}
