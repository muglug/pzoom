<?php
abstract class Atomic9 {}
final class U9 {}
final class TGen extends Atomic9 {
    /** @var non-empty-list<U9> */
    public array $type_params = [];

    public function equals(Atomic9 $other_type): bool {
        if (!$other_type instanceof self) {
            return false;
        }
        foreach ($this->type_params as $i => $type_param) {
            if ($other_type->type_params[$i] !== $type_param) { return false; }
        }
        return true;
    }
}
