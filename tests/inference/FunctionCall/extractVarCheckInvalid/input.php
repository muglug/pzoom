<?php
function takesInt(int $i): void {}

$foo = "123hello";
$a = [$foo => 15];
extract($a);
takesInt($foo);
