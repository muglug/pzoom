<?php
namespace Foo;

#[\Attribute(\Attribute::TARGET_CLASS)]
class Table {
    public function __construct(public string $name) {}
}

#[\Attribute(\Attribute::TARGET_PROPERTY)]
class Column {
    public function __construct(public string $name) {}
}

#[Table(name: "videos")]
class Video {
    #[Column(name: "id")]
    public string $id = "";

    #[Column(name: "title")]
    public string $name = "";
}

#[Table(name: "users")]
class User {
    public function __construct(
        #[Column(name: "id")]
        public string $id,

        #[Column(name: "name")]
        public string $name = "",
    ) {}
}
