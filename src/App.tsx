import { Editor } from '@/components/editor';
import '@/App.css';

function App() {
    return (
        <main className="app-container">
            <header className="app-header">
                <h1>Aurora</h1>
            </header>
            <div className="editor-wrapper">
                <Editor placeholder="Start writing your story..." />
            </div>
        </main>
    );
}

export default App;
