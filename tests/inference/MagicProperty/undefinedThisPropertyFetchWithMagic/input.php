<?php
/**
 * @property-read string $name
 * @property string $otherName
 */
class A {
    public function __get(string $name): void {
    }

    public function goodGet(): void {
        echo $this->name;
    }
    public function goodGet2(): void {
        echo $this->otherName;
    }
}
$a = new A();
echo $a->name;
echo $a->otherName;
