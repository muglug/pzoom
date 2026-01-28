<?php
$doNotContaminate = 123;

$test = 123;

$testBefore = $test;

$testInsideBefore = null;
$testInsideAfter = null;

$v = function () use (&$test, &$testInsideBefore, &$testInsideAfter, $doNotContaminate): void {
    $testInsideBefore = $test;
    $test = "test";
    $testInsideAfter = $test;

    $doNotContaminate = "test";
};
