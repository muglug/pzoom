<?php
function foo(array $arr): void {
  $r = [];
  foreach ($arr as $key => $value) {
    if ($value["foo"]) {}
    $r[] = $key;
  }
}
