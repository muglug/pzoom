<?php
/**
 * @param array<string, int> $b
 */
function a($b): int
{
  return $b["a"];
}

a(["a" => "hello"]);
