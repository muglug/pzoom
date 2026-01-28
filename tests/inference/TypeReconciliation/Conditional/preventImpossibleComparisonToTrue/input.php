<?php
/** @return false|string */
function firstChar(string $s) {
  return empty($s) ? false : $s[0];
}

if (true === firstChar("sdf")) {}
