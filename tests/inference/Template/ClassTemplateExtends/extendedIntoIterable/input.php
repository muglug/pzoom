<?php
interface ISubject {}

/**
 * @extends \IteratorAggregate<int, ISubject>
 */
interface SubjectCollection extends \IteratorAggregate
{
    /**
     * @return \Iterator<int, ISubject>
     */
    public function getIterator(): \Iterator;
}

/** @param iterable<int, ISubject> $_ */
function takesSubjects(iterable $_): void {}

function givesSubjects(SubjectCollection $subjects): void {
    takesSubjects($subjects);
}