<?php
class User
{
    public function __construct(
        protected string $name,
        protected string $problematicOne,
        protected string $id = "",
    ){}
}

new User(
name: "John",
id: "asd",
);
                
