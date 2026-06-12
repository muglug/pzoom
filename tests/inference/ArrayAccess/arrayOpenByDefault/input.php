<?php
/**
 * @param array{a: float, b: float} $params
 */
function avg(array $params): void {
  takesArrayOfFloats($params);
}

/**
 * @param array<array-key, float> $arr
 */
function takesArrayOfFloats(array $arr): void {
    foreach ($arr as $a) {
        echo $a;
    }
}

avg(["a" => 0.5, "b" => 1.5, "c" => new Exception()]);
