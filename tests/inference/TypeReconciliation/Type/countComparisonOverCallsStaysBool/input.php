<?php
class Base8 {}
class Arr8 extends Base8 {
    /** @var array{int, int} */
    public array $type_params = [1, 2];

    public function equals(Base8 $other_type): bool {
        if ($other_type::class !== static::class) {
            return false;
        }
        if (count($this->type_params) !== count($other_type->type_params)) {
            return false;
        }
        return true;
    }
}
