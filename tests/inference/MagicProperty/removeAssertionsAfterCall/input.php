<?php
/** @psalm-no-seal-properties */
class C {
    /**
     * @return mixed
     */
    public function __get(string $name) {
        return rand();
    }

    /**
     * @param mixed $value
     * @return void
     */
    public function __set(string $name, $value) {}

    public function main() : void {
        if (!isset($this->a)) {
            $this->a = ["something"];

            /**
             * @psalm-suppress MixedArrayAccess
             * @psalm-suppress MixedArgument
             */
            echo $this->a[0];
        }
    }
}
