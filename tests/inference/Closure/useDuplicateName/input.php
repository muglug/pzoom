<?php
$foo = "bar";

$a = function (string $foo) use ($foo) : string {
  return $foo;
};
