<?php
interface Colourful {
    public function color(): string;
}

enum Suit implements Colourful {
    case Hearts;
    case Diamonds;
    case Clubs;
    case Spades;

    public function color(): string {
        return match($this) {
            Suit::Hearts, Suit::Diamonds => "Red",
            Suit::Clubs, Suit::Spades => "Black",
        };
    }

    public function shape(): string {
        return "Rectangle";
    }
}

function paint(Colourful $c): void {}
function deal(Suit $s): void {
    if ($s === Suit::Clubs) {
        echo $s->color();
    }
}

paint(Suit::Clubs);
deal(Suit::Spades);

Suit::Diamonds->shape();
