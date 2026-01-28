<?php
function foo(iterable $i): array {
  if (!is_array($i)) {
    $i = iterator_to_array($i, false);
  }

  return $i;
}
