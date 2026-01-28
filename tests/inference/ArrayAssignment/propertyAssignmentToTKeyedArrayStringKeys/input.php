<?php
class Bar {
    /** @var array{a: string, b:string} */
    private array $baz = ["a" => "c", "b" => "d"];

    public function append(string $str) : void {
        $this->baz[rand(0, 1) ? "a" : "b"] = $str;
    }
}
