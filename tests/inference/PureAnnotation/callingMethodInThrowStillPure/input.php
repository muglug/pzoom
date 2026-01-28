<?php
final class MyException extends \Exception {
    public static function hello(): self
    {
        return new self();
    }
}

/**
 * @psalm-pure
 */
function sumExpectedToNotBlowPowerFuse(int $first, int $second): int {
    $sum = $first + $second;
    if ($sum > 9000) {
        throw MyException::hello();
    }
    if ($sum > 900) {
        throw new MyException();
    }
    return $sum;
}
