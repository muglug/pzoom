<?php
namespace Foo;

interface ClassMetadata {}
interface ORMClassMetadata extends ClassMetadata {}

interface EntityManagerInterface {
    public function getClassMetadata() : ClassMetadata;
}

/**
 * @method ORMClassMetadata getClassMetadata()
 * @method int getOtherMetadata()
 */
interface ORMEntityManagerInterface extends EntityManagerInterface{}

interface ConcreteEntityManagerInterface extends ORMEntityManagerInterface {}

/** @psalm-suppress InvalidReturnType */
function em(): ORMEntityManagerInterface {}
/** @psalm-suppress InvalidReturnType */
function concreteEm(): ConcreteEntityManagerInterface {}

function test(ORMClassMetadata $metadata): void {}
function test2(int $metadata): void {}

test(em()->getClassMetadata());
test(concreteEm()->getClassMetadata());

test2(em()->getOtherMetadata());
test2(concreteEm()->getOtherMetadata());
