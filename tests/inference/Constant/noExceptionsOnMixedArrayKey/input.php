<?php
function finder(string $id) : ?object {
  if (rand(0, 1)) {
    return new A();
  }

  if (rand(0, 1)) {
    return new B();
  }

  return null;
}
class A {}
class B {}
class Foo
{
    private const TYPES = [
        "type1" => A::class,
        "type2" => B::class,
    ];

    public function bar(array $data): void
    {
        if (!isset(self::TYPES[$data["type"]])) {
            throw new \InvalidArgumentException("Unknown type");
        }

        $class = self::TYPES[$data["type"]];

        $ret = finder($data["id"]);

        if (!$ret || !$ret instanceof $class) {
            throw new \InvalidArgumentException;
        }
    }
}
