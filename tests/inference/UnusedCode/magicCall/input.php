<?php
/** @psalm-no-seal-methods */
final class A {
    /** @var string */
    private $value = "default";

    /** @param string[] $args */
    public function __call(string $name, array $args) {
        if (count($args) == 1) {
            $this->modify($name, $args[0]);
        }
    }

    private function modify(string $name, string $value): void {
        call_user_func([$this, "modify" . $name], $value);
    }

    public function modifyFoo(string $value): void {
        $this->value = $value;
    }

    public function getFoo() : string {
        return $this->value;
    }
}

$m = new A();
$m->foo("value");
$m->modifyFoo("value2");
echo $m->getFoo();
