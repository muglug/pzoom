<?php
/**
 * @method static self getSelf()
 * @method $this getThis()
 */
class C {
    public static function __callStatic(string $c, array $args) {}
    public function __call(string $c, array $args) {}
}

$a = C::getSelf();
$b = (new C)->getThis();
