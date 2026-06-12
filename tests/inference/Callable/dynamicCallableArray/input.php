<?php
class A {
    /** @var string */
    private $value = "default";

    private function modify(string $name, string $value): void {
        call_user_func([$this, "modify" . $name], $value);
    }

    public function modifyFoo(string $value): void {
        $this->value = $value;
    }
}
