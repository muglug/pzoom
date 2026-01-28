<?php
$a = rand(0, 5) > 4 ? "hello" : true;

if (is_bool($a)) {
  exit;
}