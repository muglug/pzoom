<?php
final class U6 {}
final class It {
    /** @var array{U6, U6} */
    public array $type_params;
    /** @param array{U6, U6}|array<never, never> $type_params */
    public function __construct(array $type_params = []) {
        if (isset($type_params[0], $type_params[1])) {
            $this->type_params = $type_params;
        } else {
            $this->type_params = [new U6(), new U6()];
        }
    }
}
final class G {
    /** @var non-empty-list<U6> */
    public array $type_params = [];
}
function f(G $g): It {
    if (count($g->type_params) > 2) {
        throw new \InvalidArgumentException('Too many');
    }
    return new It($g->type_params);
}
