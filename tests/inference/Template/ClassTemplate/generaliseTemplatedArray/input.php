<?php
/** @template TData */
class Container {
    /** @var TData */
    public $data;

    /** @param TData $data */
    public function __construct($data) {
        $this->data = $data;
    }
}

/** @param Container<array{name: string}> $r */
function takesContainer(Container $r): void {
    $r->data = ["name" => "David"];
}

$me = new Container(["name" => "Matthew"]);

takesContainer($me);

if ($me->data["name"] === "David") {}