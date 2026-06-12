<?php
/**
 * @psalm-type A callable(int, int): string
 * @psalm-type B callable(int, int=): string
 * @psalm-type C callable(int $a, string $b): void
 * @psalm-type D callable(string $c): mixed
 * @psalm-type E callable(string $c): mixed
 * @psalm-type F callable(float...): (int|null)
 * @psalm-type G callable(float ...$d): (int|null)
 * @psalm-type H callable(array<int>): array<string>
 * @psalm-type I callable(array<string, int> $e): array<int, string>
 * @psalm-type J callable(array<int> ...): string
 * @psalm-type K callable(array<int> ...$e): string
 * @psalm-type L \Closure(int, int): string
 *
 * @method ma(): A
 * @method mb(): B
 * @method mc(): C
 * @method md(): D
 * @method me(): E
 * @method mf(): F
 * @method mg(): G
 * @method mh(): H
 * @method mi(): I
 * @method mj(): J
 * @method mk(): K
 * @method ml(): L
 */
class Foo {
    public function __call(string $method, array $params) { return 1; }
}

$foo = new \Foo();
$output_ma = $foo->ma();
$output_mb = $foo->mb();
$output_mc = $foo->mc();
$output_md = $foo->md();
$output_me = $foo->me();
$output_mf = $foo->mf();
$output_mg = $foo->mg();
$output_mh = $foo->mh();
$output_mi = $foo->mi();
$output_mj = $foo->mj();
$output_mk = $foo->mk();
$output_ml = $foo->ml();
