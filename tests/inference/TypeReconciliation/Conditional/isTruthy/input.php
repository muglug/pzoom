<?php
function f(string $s = null): string {
  if ($s == true) {
      return $s;
  }

  return "backup";
}