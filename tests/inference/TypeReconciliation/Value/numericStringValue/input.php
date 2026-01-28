<?php
/** @psalm-return numeric-string */
function makeNumeric() : string {
  return "12.34";
}

/** @psalm-param numeric-string $string */
function consumeNumeric(string $string) : void {
  \error_log($string);
}

consumeNumeric("12.34");