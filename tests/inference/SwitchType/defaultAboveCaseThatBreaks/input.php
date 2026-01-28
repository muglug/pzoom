<?php
function foo(string $a) : string {
  switch ($a) {
    case "a":
      return "hello";

    default:
    case "b":
      break;

    case "c":
      return "goodbye";
  }
}
