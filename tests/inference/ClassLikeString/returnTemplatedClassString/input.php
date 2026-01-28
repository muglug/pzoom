<?php
/**
 * @template T
 *
 * @param class-string<T> $shouldBe
 * @return class-string<T>
 */
function identity(string $shouldBe) : string {  return $shouldBe; }

identity(DateTimeImmutable::class)::createFromMutable(new DateTime());
