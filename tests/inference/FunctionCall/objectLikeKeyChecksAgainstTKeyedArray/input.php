<?php
/**
 * @param array{a: string} $b
 */
function a($b): string
{
  return $b["a"];
}

a(["a" => "hello"]);
