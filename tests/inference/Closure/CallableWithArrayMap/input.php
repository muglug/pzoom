<?php
/**
 * @psalm-template T
 * @param class-string<T> $className
 * @return callable(...mixed):T
 */
function maker(string $className) {
   return function(...$args) use ($className) {
      /** @psalm-suppress MixedMethodCall */
      return new $className(...$args);
   };
}
$maker = maker(stdClass::class);
$result = array_map($maker, ["abc"]);
