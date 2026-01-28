<?php
$e = rand(0, 10)
  ? new RuntimeException("m")
  : null;

if ($e instanceof Exception) {
  echo "good";
}
