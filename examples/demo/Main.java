package demo;

public class Main {
    static class Book {
        String title;
        String author;
        boolean borrowed;

        Book(String title, String author) {
            this.title = title;
            this.author = author;
            this.borrowed = false;
        }

        String status() {
            if (borrowed) {
                return "borrowed";
            }
            return "available";
        }
    }

    static class Member {
        String name;
        int borrowedCount;

        Member(String name) {
            this.name = name;
            this.borrowedCount = 0;
        }
    }

    static class Library {
        Book[] books;
        int size;

        Library(int capacity) {
            this.books = new Book[capacity];
            this.size = 0;
        }

        void addBook(Book book) {
            books[size] = book;
            size = size + 1;
        }

        Book findBook(String title) {
            int i = 0;
            while (i < size) {
                if (books[i].title.equals(title)) {
                    return books[i];
                }
                i = i + 1;
            }
            return null;
        }

        void borrowBook(String title, Member member) {
            Book book = findBook(title);
            if (book == null) {
                System.out.println("Book not found: " + title);
                return;
            }
            if (book.borrowed) {
                System.out.println(title + " is already borrowed.");
                return;
            }
            book.borrowed = true;
            member.borrowedCount = member.borrowedCount + 1;
            System.out.println(member.name + " borrowed " + title + ".");
        }

        void returnBook(String title, Member member) {
            Book book = findBook(title);
            if (book == null) {
                System.out.println("Book not found: " + title);
                return;
            }
            if (!book.borrowed) {
                System.out.println(title + " was not borrowed.");
                return;
            }
            book.borrowed = false;
            member.borrowedCount = member.borrowedCount - 1;
            System.out.println(member.name + " returned " + title + ".");
        }

        void printCatalog() {
            int i = 0;
            System.out.println("Library catalog:");
            while (i < size) {
                Book book = books[i];
                System.out.println("- " + book.title + " by " + book.author + " [" + book.status() + "]");
                i = i + 1;
            }
        }
    }

    public static void main(String[] args) {
        Library library = new Library(5);
        library.addBook(new Book("The Hobbit", "J.R.R. Tolkien"));
        library.addBook(new Book("Clean Code", "Robert C. Martin"));
        library.addBook(new Book("The Pragmatic Programmer", "Andrew Hunt"));

        Member alice = new Member("Alice");
        Member bob = new Member("Bob");

        library.printCatalog();
        System.out.println("----");

        library.borrowBook("The Hobbit", alice);
        library.borrowBook("The Hobbit", bob);
        library.borrowBook("Clean Code", bob);
        System.out.println("Alice has " + alice.borrowedCount + " book(s).");
        System.out.println("Bob has " + bob.borrowedCount + " book(s).");
        System.out.println("----");

        library.returnBook("The Hobbit", alice);
        library.borrowBook("The Hobbit", bob);
        System.out.println("Alice has " + alice.borrowedCount + " book(s).");
        System.out.println("Bob has " + bob.borrowedCount + " book(s).");
        System.out.println("----");

        library.printCatalog();
    }
}
