<?php
/**
 * @param array{a: int} $b
 */
function a($b): int
{
  return $b["a"];
}

a(["a" => "hello"]);
