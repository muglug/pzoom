<?php
$a = rand(0, 5) > 4 ? "hello" : 5;

if (is_numeric($a)) {
  exit;
}