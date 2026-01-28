<?php
class C {
  /** @var array */
  public $i;

  public function __construct() {
    $this->i = [
      function (Exception $e): void {},
      function (LogicException $e): void {},
    ];
  }
}
