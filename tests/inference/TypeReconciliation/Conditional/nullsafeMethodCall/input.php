<?php
class IntLinkedList {
    public function __construct(
        public int $value,
        private ?self $next
    ) {}

    public function getNext() : ?self {
        return $this->next;
    }
}

function skipOne(IntLinkedList $l) : ?int {
    return $l->getNext()?->value;
}

function skipTwo(IntLinkedList $l) : ?int {
    return $l->getNext()?->getNext()?->value;
}