<?php

class Animal {}
final class Dog extends Animal {}

/** @template-covariant T */
interface Source {}

/** @template-contravariant T */
interface Sink {}

/** @template T */
final class Cell {}

/** @implements Source<Dog> */
final class DogSource implements Source {}

/** @param Source<Animal> $source */
function readAnimal(Source $source): void {}

/** @param Source<Dog> $source */
function readDog(Source $source): void {}

/** @param Sink<Dog> $sink */
function writeDog(Sink $sink): void {}

/** @param Sink<Animal> $sink */
function writeAnimal(Sink $sink): void {}

/** @param Cell<Animal> $cell */
function keepAnimal(Cell $cell): void {}

/**
 * @param Source<Animal> $animals
 * @param Sink<Animal> $animalSink
 * @param Sink<Dog> $dogSink
 * @param Cell<Dog> $dogs
 */
function applyVariance(Source $animals, Sink $animalSink, Sink $dogSink, Cell $dogs): void
{
    readAnimal(new DogSource());
    readDog($animals);
    writeDog($animalSink);
    writeAnimal($dogSink);
    keepAnimal($dogs);
}
