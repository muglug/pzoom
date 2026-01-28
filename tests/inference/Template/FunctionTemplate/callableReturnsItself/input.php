<?php
$a =
  /**
   * @param callable():string $s
   * @return string
   */
  function(callable $s) {
    return $s();
  };

/**
 * @template T1
 * @param callable(callable():T1):T1 $s
 * @return void
 */
function takesReturnTCallable(callable $s) {}

takesReturnTCallable($a);