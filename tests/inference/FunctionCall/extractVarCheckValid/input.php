<?php
function takesInt(int $i): void {}

$foo = "foo";
$a = [$foo => 15];
extract($a);
takesInt($foo);
