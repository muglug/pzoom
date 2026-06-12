<?php
/**
 * @param array<string, string> $b
 */
function a($b): string
{
  return $b["a"];
}

a(["a" => "hello"]);
