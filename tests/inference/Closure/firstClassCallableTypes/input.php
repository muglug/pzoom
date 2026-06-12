<?php
class A {
    public function isOdd(int $i): bool { return (bool)($i % 2); }

    /**
     * @param list<int> $l
     * @return list<int>
     */
    public function f(array $l): array {
        return array_values(array_filter($l, $this->isOdd(...)));
    }
}
function g(string $s): bool { return $s !== ''; }
/** @param list<string> $l */
function h(array $l): array {
    return array_filter($l, g(...));
}
