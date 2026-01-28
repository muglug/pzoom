<?php
$fn = function(int $a): void{};
function a(callable $fn): void{
  $fn(++$a);
}
a($fn);
