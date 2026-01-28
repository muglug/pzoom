<?php
function takesString(string $s): void {
  if (!is_numeric($s) || empty($s)) {}
}