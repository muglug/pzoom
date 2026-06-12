<?php
const K = 5;

abstract class C6 {

  public const M = [
    1 => -1,
    K => 6,
  ];

  /**
   * @param int $k
   */
  public static function f(int $k): void {
      $a = self::M;
      print_r($a);
  }

}
