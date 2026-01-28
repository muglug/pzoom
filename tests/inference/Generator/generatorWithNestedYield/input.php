<?php
function other_generator(): Generator {
  yield "traffic";
  return 1;
}
function foo(): Generator {
  /** @var int */
  $value = yield from other_generator();
  var_export($value);
}
