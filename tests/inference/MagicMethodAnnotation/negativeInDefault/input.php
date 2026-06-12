<?php
/**
 * @method void foo($a = -0.1, $b = -12)
 */
class G
{
    public function __call(string $method, array $attributes): void
    {
    }
}
(new G)->foo();
