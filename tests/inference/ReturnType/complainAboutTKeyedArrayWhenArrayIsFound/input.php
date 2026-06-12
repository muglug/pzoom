<?php
/** @return array{a:string,b:string,c:string} */
function foo(): array {
  $arr = [];
  foreach (["a", "b"] as $key) {
    $arr[$key] = "foo";
  }
  return $arr;
}
