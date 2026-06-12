<?php
/**
 * @template T as int
 * @param T $i
 * @return (T is 1 ? string : (T is 2 ? int : (T is 3 ? float : bool)))
 */
function h(int $i) { if ($i === 1) { return "x"; } if ($i === 2) { return 5; } if ($i === 3) { return 1.5; } return true; }

/** @return float */
function t3() { return h(3); }
/** @return int */
function t2() { return h(2); }
/** @return string */
function t1() { return h(1); }
echo t1() . t2() . t3();
