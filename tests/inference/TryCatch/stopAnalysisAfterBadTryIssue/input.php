<?php
$foo = true;

try {
  $a->bar();
} catch (\TypeError $e) {
  $foo = false;
}

if (!$foo) {}
