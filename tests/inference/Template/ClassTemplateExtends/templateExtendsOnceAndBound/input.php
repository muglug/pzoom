<?php
/** @template T1 */
class Repo {
    /** @return ?T1 */
    public function findOne() {
        return null;
    }
}

class SpecificEntity {}

/** @template-extends Repo<SpecificEntity> */
class AnotherRepo extends Repo {}

$a = new AnotherRepo();
$b = $a->findOne();
