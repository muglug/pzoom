<?php
class ParentClass {
    /**
     * @psalm-suppress MissingParamType
     * @return mixed
     */
    public function func($var) {
        return $var;
    }
}

/** @psalm-suppress MethodSignatureMismatch */
class MismatchMethod extends ParentClass {
    /** @return mixed */
    public function func(string $var) {
        return $var;
    }
}
                
